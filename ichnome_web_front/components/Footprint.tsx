import { IchFootprint } from "@/api/types";
import FootprintLink from "./FootprintLink";
import Digest from "./Digest";

type Props = {
  footprint: IchFootprint;
};

export const Footprint: React.FC<Props> = ({ footprint: { digest, size, git_object_id } }) => {
  return (
    <ul>
      <li>
        Digest: <FootprintLink digest={digest} />
      </li>
      <li>Size: {size}</li>
      <li>
        Git Footprint ID: <Digest digest={git_object_id} />
      </li>
    </ul>
  );
};

export default Footprint;
